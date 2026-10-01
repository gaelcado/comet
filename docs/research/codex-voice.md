# Native Codex subscription voice

Zeron connects a local Codex chat to the installed Codex voice runtime. Voice
uses ChatGPT authentication and the normal Codex plan budget. Codex may use
additional credits according to the user's account settings and usage limits.
There is no OpenAI API-key fallback, separate public Realtime API client, or
promise of an included-quota-only spending cap.

Sources checked 2026-10-01:

- [App-server](https://learn.chatgpt.com/docs/app-server)
- [Voice](https://learn.chatgpt.com/docs/features/voice)
- [Pricing](https://learn.chatgpt.com/docs/pricing#how-much-does-voice-cost)
- [Official Codex source, rust-v0.159.0](https://github.com/openai/codex/tree/377f7f557a6bdea0f3a2d26d4d899c66db4789d0/codex-rs),
  commit `377f7f557a6bdea0f3a2d26d4d899c66db4789d0`: app-server realtime schemas,
  `core/src/realtime_conversation.rs`, `realtime-webrtc`, and `voice-host`.

## Transport and authentication

Codex 0.159's WebSocket audio path calls `realtime_api_key` and requires API-key
credentials, including an environment-key fallback. That path is unsuitable for
this integration. Native subscription voice uses **WebRTC V3** instead:

1. Prepare or reuse an idle native thread with the normal Zeron MCP server.
   No initial `turn/start`, empty user message, title request, or submitted draft.
2. Check `account/read` on that same process for `chatgpt` authentication, reject
   custom providers and realtime backend overrides, inspect ordinary usage
   permission, and obtain `thread/realtime/listVoices`.
3. Resolve the helper inside the physical standalone Codex package (layout 1,
   version 0.159 or later). Its protocol/build handshake must succeed.
4. Initialize the helper and gather its SDP offer. Call `thread/realtime/start`
   with WebRTC, V3, audio output, the lease session id, startup context, and
   `clientManagedHandoffs: false`. Codex owns background task delegation.
5. The request response acknowledges submission. Independently wait for both
   `thread/realtime/started` and `thread/realtime/sdp`; validate session/version.
6. Apply the answer and wait for transport readiness. Open default local audio
   devices while muted/suppressed. Only enable them for an attached owner.

Audio, Opus, resampling, echo cancellation, reference audio, native interruption,
and bounded playout remain in Codex's helper. No PCM or SDP reaches the UI,
document, sync journal, or owner RPC stream. Helper stderr is discarded; errors
and diagnostics use typed rejection reasons. Child processes are killed and
reaped on cancellation, including when a helper control reply is stalled.

The earlier `zeron-audio` PCM/AEC prototype remains isolated and tested offline;
it is not a dependency of the released UI or the subscription voice path.
`voice-experimental` is retained as an empty compatibility build feature. The
Codex voice control is available in ordinary desktop builds.

## Lifecycle, transcript and MCP

One engine lease owns local voice. Tokens are redacted from Debug. Ownership is
exclusive, generation checked, ephemeral, and cancelled when its scoped stream
is dropped. A five-second unattached-owner watchdog prevents abandoned starts
from creating a provider call. A successor waits for native stop and its terminal
notification; late events cannot acquire the successor's generation. Native
capture termination is independent of the actor's pending control I/O. The
stdout reader also cancels capture on identity changes, overflow and EOF; it
does not wait for a blocked audio-control reply. Identity retirement invalidates
pending start reservations as well as existing leases. The physical Codex
release is pinned before spawning app-server so an installer symlink change
cannot select a different helper for a warm runtime.

Voice protects the warm runtime from idle reaping and updates. Voice stop leaves
delegated Codex work running. Account/profile retirement, navigation, window
owner loss, audio failure and provider termination close voice. An agent question
restores the text composer so the usual input UI can answer it. Reconnection and
microphone resumption require a fresh user action.

Canonical `thread/realtime/item/completed` transcript segments are committed by
the serialized document owner, deduplicated by session/item identity. Legacy
transcript events and partial text never become durable messages. Native BEM
promotion uses the existing Codex task transcript; it does not resubmit a spoken
request or create another task response. Native `turn/started` publishes the
engine turn boundary before its text/tools, including on an idle bootstrap.

The engine-injected MCP origin defaults voice-created chats to Codex. Existing
chat delivery and derived local tasks require Codex/ChatGPT authentication;
other providers and unverified remote execution are rejected before forwarding.
Read-only status queries remain available. This subscription policy stays with
continuing tasks after voice ends; ordinary text MCP defaults are preserved.

## Interface and packaging

During Starting, Active and Stopping, the composer is replaced by a central
orb extracted from Bezel at `6141af9c16f7353cdf36003f7404e0a94566a163`.
Bezel is not a dependency. MIT notices accompany the extracted component.
The orb tracks listening, speaking and task work, responds to level meters,
uses the theme and reduced-motion setting, and stops hidden/background timers.
Controls distinguish microphone mute, ending voice and stopping a task.

New voice chats preserve drafts and attachments without uploading or submitting
them. Current, reused and fresh worktree selections prepare the normal workspace
before idle startup. Native voice ids from the provider catalog can be selected
for the next session; only that preference is persisted. Devices use the operating
system defaults. Codex's current helper protocol does not expose device selection.

The native helper/runtime is supplied by the standalone Codex installation,
not redistributed by Zeron. npm/CLI-only installations without those resources
report `nativeRuntimeUnavailable`. macOS production/dev bundles declare microphone
usage and audio-input entitlements. No new DSP DLLs or C++ build dependency are
added to the production UI. Apache-2.0 attribution for the adapted native helper
protocol is included in `THIRD_PARTY_NOTICES.md`.

## Validation and practical limits

Offline CI never starts a provider session or microphone. Fake app-server and
helper peers exercise actual engine RPC, independent start acknowledgements,
SDP signaling, owner attachment, native controls, transcript replay, task
continuation, stop/restart and authentication rejection. Synthetic audio tests
remain offline. Cross-platform checks compile the native bridge without loading
hardware; signed package/device trials require their actual operating systems.

Read-only discovery:

```sh
python3 scripts/codex-voice-probe.py
```

Explicit opt-in connectivity smoke (consumes normal Codex voice usage, does not
open microphone devices):

```sh
python3 scripts/codex-voice-smoke.py --live
```

Optional device/playout check opens default devices with the microphone **muted**
and requests a short greeting:

```sh
python3 scripts/codex-voice-smoke.py --live --devices
```

Evidence on 2026-10-01, Linux, installed Codex 0.159.0: ChatGPT authentication,
packaged helper initialization, real V3 WebRTC negotiation, transport readiness
and native stop passed. No API fallback was used. The optional device trial
failed at opening audio devices: this environment exposes a network output
monitor, without a usable default microphone. No real speech or acoustic echo
trial is claimed. macOS/Windows signed package and physical-device trials also
remain unverified; protocol/fixture success does not establish those results.

Implementation review and local test results are recorded in the ignored
`.personal/codex-voice/` working notes. No push, deployment or remote PR is part
of this task.
